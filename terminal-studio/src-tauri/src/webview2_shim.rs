//! WebView2Loader 静态库的 MSVC CRT 符号垫片。
//!
//! WebView2LoaderStatic.lib 由 MSVC 编译, 引用 MSVC C++ 运行时的
//! 内存运算符; GNU 工具链下无这些符号, 这里按 MSVC x64 修饰名
//! 提供实现(转发到 mingw crt 的 malloc/free), 完成纯静态链接。

use std::ffi::c_void;

unsafe extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn free(ptr: *mut c_void);
}

/// operator new(unsigned __int64) — `??2@YAPEAX_K@Z`
#[export_name = "??2@YAPEAX_K@Z"]
pub unsafe extern "C" fn msvc_operator_new(size: u64) -> *mut c_void {
    malloc(size as usize)
}

/// operator new(unsigned __int64, std::nothrow_t const&) —
/// `??2@YAPEAX_KAEBUnothrow_t@std@@@Z`; nothrow 语义 = 失败返回空指针,
/// malloc 天然满足
#[export_name = "??2@YAPEAX_KAEBUnothrow_t@std@@@Z"]
pub unsafe extern "C" fn msvc_operator_new_nothrow(size: u64, _tag: *const c_void) -> *mut c_void {
    malloc(size as usize)
}

/// std::nothrow 标签对象 — `?nothrow@std@@3Unothrow_t@1@B`;
/// 只取地址作区分标签, 值从不被读
#[export_name = "?nothrow@std@@3Unothrow_t@1@B"]
pub static MSVC_NOTHROW: u8 = 0;

/// MSVC /GS 栈保护 cookie — 固定默认值(MSVC 默认值, 正常由
/// __security_init_cookie 随机化, 固定值只影响防御强度不影响功能)
#[export_name = "__security_cookie"]
pub static MSVC_SECURITY_COOKIE: u64 = 0x2B99_2DDF_A232_49F5;

/// MSVC /GS 栈保护校验: cookie 不匹配即终止进程
#[export_name = "__security_check_cookie"]
pub unsafe extern "C" fn msvc_security_check_cookie(cookie: u64) {
    if cookie != MSVC_SECURITY_COOKIE {
        std::process::abort();
    }
}

/// MSVC 魔术静态(函数内局部 static)线程安全初始化的世代计数。
/// 约定: 每个 static 配一个守护变量 TEP, TEP >= epoch 表示已初始化。
#[export_name = "_Init_thread_epoch"]
pub static MSVC_INIT_THREAD_EPOCH: i32 = 0;

/// 进入初始化: 标记 TEP = -1(未完成), 调用方据此判断是否需要构造
#[export_name = "_Init_thread_header"]
pub unsafe extern "C" fn msvc_init_thread_header(tep: *mut i32) {
    if !tep.is_null() {
        *tep = -1;
    }
}

/// 完成初始化: 记录完成世代。webview 加载器仅在主线程单次执行,
/// 单线程语义下 *TEP = epoch 即可满足调用方的比较判断
#[export_name = "_Init_thread_footer"]
pub unsafe extern "C" fn msvc_init_thread_footer(tep: *mut i32, epoch: i32) {
    if !tep.is_null() {
        *tep = epoch;
    }
}

/// operator delete(void*) — `??3@YAXPEAX@Z`
#[export_name = "??3@YAXPEAX@Z"]
pub unsafe extern "C" fn msvc_operator_delete(ptr: *mut c_void) {
    free(ptr);
}

/// operator delete(void*, unsigned __int64) — `??3@YAXPEAX_K@Z`
#[export_name = "??3@YAXPEAX_K@Z"]
pub unsafe extern "C" fn msvc_operator_delete_sized(ptr: *mut c_void, _size: u64) {
    free(ptr);
}
