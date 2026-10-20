//! Aggregate persisted logs; independent of the monitor's capped UI buffer.
use chrono::{Duration, NaiveDateTime, Timelike};
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use std::collections::BTreeMap;
use crate::error::{AppError, AppResult};

#[derive(Debug, Serialize)]
pub struct TrafficCount {
    pub received: i64,
    pub sent: i64,
}

#[derive(Debug, Serialize)]
pub struct TrafficBucket {
    pub start: String,
    pub end: String,
    #[serde(flatten)]
    pub count: TrafficCount,
}

#[derive(Debug, Serialize)]
pub struct ChannelTraffic {
    pub node_id: Option<i64>,
    pub name: String,
    #[serde(flatten)]
    pub count: TrafficCount,
}

#[derive(Debug, Serialize)]
pub struct TrafficStats {
    pub start: String,
    pub end: String,
    pub buckets: Vec<TrafficBucket>,
    pub channels: Vec<ChannelTraffic>,
    #[serde(flatten)]
    pub count: TrafficCount,
}

fn stamp(date: NaiveDateTime) -> String {
    date.format("%Y-%m-%d %H:%M:%S").to_string()
}

pub async fn aggregate(pool: &SqlitePool, range: &str, now: NaiveDateTime) -> AppResult<TrafficStats> {
    let (seconds, steps) = match range {
        "1h" => (300_i64, 12),
        "24h" => (3600, 24),
        "7d" => (86400, 7),
        _ => return Err(AppError::msg("不支持的统计时段")),
    };
    // Logs are stored as local wall-clock timestamps. Use that same convention
    // for both bounds and bucket arithmetic (no implicit UTC conversion).
    let end = now.with_nanosecond(0).unwrap() + Duration::seconds(1);
    let start = end - Duration::seconds(seconds * steps);
    let mut stats = TrafficStats {
        start: stamp(start),
        end: stamp(end),
        buckets: (0..steps).map(|i| TrafficBucket {
            start: stamp(start + Duration::seconds(i * seconds)),
            end: stamp(start + Duration::seconds((i + 1) * seconds)),
            count: TrafficCount { received: 0, sent: 0 },
        }).collect(),
        channels: Vec::new(),
        count: TrafficCount { received: 0, sent: 0 },
    };
    let rows = sqlx::query(
        "SELECT CAST((CAST(strftime('%s', l.ts) AS INTEGER) - ?) / ? AS INTEGER) AS bucket,
                l.node_ref, n.name,
                SUM(CASE WHEN l.direction='in' THEN 1 ELSE 0 END) AS received,
                SUM(CASE WHEN l.direction='out' THEN 1 ELSE 0 END) AS sent
         FROM logs l LEFT JOIN nodes n ON n.id=l.node_ref
         WHERE l.ts >= ? AND l.ts < ? AND l.direction IN ('in','out')
         GROUP BY bucket, l.node_ref, n.name"
    )
    .bind(start.and_utc().timestamp()).bind(seconds)
    .bind(&stats.start).bind(&stats.end)
    .fetch_all(pool).await?;
    let mut channels = BTreeMap::<Option<i64>, ChannelTraffic>::new();
    for row in rows {
        let index: i64 = row.try_get("bucket")?;
        let Some(bucket) = usize::try_from(index).ok().and_then(|i| stats.buckets.get_mut(i)) else { continue };
        let received: i64 = row.try_get("received")?;
        let sent: i64 = row.try_get("sent")?;
        let node_id: Option<i64> = row.try_get("node_ref")?;
        let name: Option<String> = row.try_get("name")?;
        bucket.count.received += received;
        bucket.count.sent += sent;
        stats.count.received += received;
        stats.count.sent += sent;
        let channel = channels.entry(node_id).or_insert_with(|| ChannelTraffic {
            node_id,
            name: name.unwrap_or_else(|| node_id.map(|id| format!("已删除通道 #{id}")).unwrap_or_else(|| "未关联通道".into())),
            count: TrafficCount { received: 0, sent: 0 },
        });
        channel.count.received += received;
        channel.count.sent += sent;
    }
    stats.channels = channels.into_values().collect();
    stats.channels.sort_by(|a, b| (b.count.received + b.count.sent).cmp(&(a.count.received + a.count.sent)).then(a.node_id.cmp(&b.node_id)));
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE nodes(id INTEGER PRIMARY KEY,name TEXT); CREATE TABLE logs(ts TEXT,direction TEXT,node_ref INTEGER); CREATE INDEX idx_logs_ts ON logs(ts);")
            .execute(&pool).await.unwrap();
        pool
    }
    fn now() -> NaiveDateTime { NaiveDateTime::parse_from_str("2026-10-20 10:59:59", "%Y-%m-%d %H:%M:%S").unwrap() }

    #[tokio::test]
    async fn traffic_counts_boundaries_buckets_and_channels() {
        let p = pool().await;
        sqlx::query("INSERT INTO nodes VALUES(1,'通道 A'),(2,'通道 A'); INSERT INTO logs VALUES
            ('2026-10-20 09:59:59.999','in',1),
            ('2026-10-20 10:00:00.000','in',1),
            ('2026-10-20 10:04:59.999','out',1),
            ('2026-10-20 10:05:00.000','in',2),
            ('2026-10-20 10:59:59.999','out',NULL),
            ('2026-10-20 10:30:00.000','out',9),
            ('2026-10-20 11:00:00.000','in',1)")
            .execute(&p).await.unwrap();
        let s = aggregate(&p, "1h", now()).await.unwrap();
        assert_eq!((s.count.received, s.count.sent), (2, 3));
        assert_eq!(s.buckets.len(), 12);
        assert_eq!((s.buckets[0].count.received, s.buckets[0].count.sent), (1, 1));
        assert_eq!(s.buckets[1].count.received, 1);
        assert_eq!(s.buckets[2].count.received + s.buckets[2].count.sent, 0);
        assert_eq!(s.buckets[11].count.sent, 1);
        assert_eq!(s.channels.len(), 4); // duplicate names don't merge identities
        assert_eq!(s.channels[0].node_id, Some(1));
        assert!(s.channels.iter().any(|c| c.name == "未关联通道"));
        assert!(s.channels.iter().any(|c| c.name == "已删除通道 #9"));
    }

    #[tokio::test]
    async fn traffic_empty_ranges_and_validation() {
        let p = pool().await;
        for (r, n) in [("1h", 12), ("24h", 24), ("7d", 7)] {
            let s = aggregate(&p, r, now()).await.unwrap();
            assert_eq!(s.buckets.len(), n);
            assert_eq!(s.count.received + s.count.sent, 0);
            assert!(s.channels.is_empty());
        }
        assert!(aggregate(&p, "all", now()).await.is_err());
    }
}
