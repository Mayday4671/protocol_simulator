import {afterEach, beforeEach, expect, it, vi} from 'vitest';
import {cleanup, fireEvent, render, screen} from '@testing-library/react';
import PacketInspector from './PacketInspector';

beforeEach(() => {
  Object.defineProperty(window, 'matchMedia', {writable: true, value: () => ({matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {}})});
  vi.stubGlobal('ResizeObserver', class {observe() {} unobserve() {} disconnect() {}});
  const getComputedStyle = window.getComputedStyle.bind(window);
  vi.spyOn(window, 'getComputedStyle').mockImplementation(element => getComputedStyle(element));
});
afterEach(() => {cleanup(); vi.restoreAllMocks();});

it('在两个解析面板中选择字段只定位该面板的字节', () => {
  render(<><PacketInspector hex="01" decoded={{fields: [{path: 'a', labelZh: '预览字段', startOffset0: 0, byteLength: 1, rawHex: '01'}]}}/><PacketInspector hex="02" decoded={{fields: [{path: 'b', labelZh: '实际报文字段', startOffset0: 0, byteLength: 1, rawHex: '02', decimal: 2, meaningZh: '执行成功'}]}}/></>);
  const bytes = screen.getAllByRole('button', {name: /^(01|02)$/});
  const scrollPreview = vi.fn();
  const scrollActual = vi.fn();
  bytes[0].scrollIntoView = scrollPreview;
  bytes[1].scrollIntoView = scrollActual;
  fireEvent.click(screen.getByText('实际报文字段'));
  expect(scrollActual).toHaveBeenCalledOnce();
  expect(scrollPreview).not.toHaveBeenCalled();
  expect(bytes[1].classList.contains('highlighted')).toBe(true);
  expect(bytes[0].classList.contains('highlighted')).toBe(false);
  const values = document.querySelector('.selected-field-values');
  expect(values?.textContent).toContain('HEX02');
  expect(values?.textContent).toContain('DEC2');
  expect(values?.textContent).toContain('含义执行成功');
});
