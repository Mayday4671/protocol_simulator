// Real XmlEditor component, isolated React state. No application data or UDP access.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const longXml = ['<?xml version="1.0" encoding="UTF-8"?>', '<protocols>', ...Array.from({ length: 1000 }, (_, index) => `  <protocol id="p${index + 1}" name="无人机协议${index + 1}"><field name="设备地址" value="${'AB'.repeat(index === 777 ? 220 : 3)}" /></protocol>`), '</protocols>'].join('\n');
const fixture = `<!doctype html><html><head><meta charset="UTF-8"></head><body><div id="root"></div><script type="module">
  import RefreshRuntime from '/@react-refresh';
  RefreshRuntime.injectIntoGlobalHook(window); window.$RefreshReg$ = () => {}; window.$RefreshSig$ = () => type => type; window.__vite_plugin_react_preamble_installed__ = true;
  const {default: React} = await import('/node_modules/.vite/deps/react.js');
  const {default: ReactDOM} = await import('/node_modules/.vite/deps/react-dom_client.js');
  await import('/src/styles/globals.css');
  const {XmlEditor} = await import('/src/components/protocols/XmlEditor.tsx');
  const h=React.createElement;
  function Fixture() {
    const [value,setValue]=React.useState('<protocols>\\n  <protocol name="无人机" />\\n</protocols>');
    const editor=React.useRef(null);
    window.__xmlRef=editor;
    window.__xmlValue=value;
    const update=React.useCallback(next=>{window.__xmlChanges.push({time:performance.now(),value:next});setValue(next);},[]);
    const validated=React.useCallback(check=>{window.__xmlChecks.push({time:performance.now(),check});},[]);
    return h('main',{className:'flex h-screen min-h-0 flex-col p-4 gap-3'},
      h('nav',{className:'flex gap-3'},h('button',{'aria-label':'载入短XML',onClick:()=>setValue('<protocols>\\n  <protocol name="无人机" />\\n</protocols>')},'短文档'),
        h('button',{'aria-label':'载入空节点XML',onClick:()=>setValue('<root>\\n\\n</root>')},'空节点'),
        h('button',{'aria-label':'载入长XML',onClick:()=>setValue(window.__longXml)},'长文档')),
      h(XmlEditor,{ref:editor,value,onChange:update,onValidationChange:validated}));
  }
  window.__xmlChecks=[]; window.__xmlChanges=[];
  ReactDOM.createRoot(document.getElementById('root')).render(h(Fixture));
</script></body></html>`;

(async () => {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  page.setDefaultTimeout(10000);
  const out = process.env.UI_ARTIFACT_DIR || path.resolve('artifacts/xml-editor');
  fs.mkdirSync(out, { recursive: true });
  const errors = [];
  page.on('pageerror', error => errors.push(String(error)));
  await page.addInitScript(value => { window.__longXml=value; },longXml);
  await page.route('**/__xml_editor_fixture', route => route.fulfill({ contentType:'text/html',body:fixture }));
  const results=[];
  const failures=[];
  const pass=name=>{results.push(name);console.log('PASS: '+name);};
  try {
    await page.goto((process.env.UI_BASE_URL || 'http://127.0.0.1:1422')+'/__xml_editor_fixture');
    if (process.argv.includes('--legacy')) {
      const editor=page.locator('textarea');
      await editor.waitFor();
      await editor.fill('<protocols>\n</protocols>\n<');
      const tail=await page.evaluate(()=>({input:document.querySelector('textarea').value,paint:document.querySelector('pre').textContent}));
      const missingTail=tail.paint.replace(/\n$/,'')!==tail.input;
      assert.equal(missingTail,true,'legacy evidence expects the highlighted text to lose a standalone <');
      await page.screenshot({path:path.join(out,'legacy-missing-opening-bracket.png'),fullPage:true});
      await editor.fill('<root>\n\n</root>');
      await page.keyboard.press('Control+Home');
      await page.keyboard.press('ArrowDown');
      await page.keyboard.press('Tab');
      await page.keyboard.type('hello');
      const typed=await editor.inputValue();
      await page.keyboard.press('Control+z');
      const undoText=await editor.inputValue();
      await page.keyboard.press('Control+z');
      const undoIndent=await editor.inputValue();
      await page.getByRole('button',{name:'载入长XML'}).click();
      await editor.focus();
      await page.keyboard.press('Control+End');
      const metrics=await page.evaluate(()=>{
        const ta=document.querySelector('textarea');const pre=document.querySelector('pre');
        const style=el=>{const cs=getComputedStyle(el);return {fontFamily:cs.fontFamily,fontSize:cs.fontSize,lineHeight:cs.lineHeight,letterSpacing:cs.letterSpacing,scrollHeight:el.scrollHeight,scrollTop:el.scrollTop,scrollWidth:el.scrollWidth,clientHeight:el.clientHeight};};
        return {input:style(ta),paint:style(pre),lineCount:ta.value.split('\n').length};
      });
      await page.screenshot({path:path.join(out,'legacy-overlay-long.png'),fullPage:true});
      fs.writeFileSync(path.join(out,'legacy-red.json'),JSON.stringify({missingTail,tail,undo:{typed,undoText,undoIndent},metrics,pageErrors:errors},null,2));
      console.log('EXPECTED LEGACY FAILURE: standalone < disappears from visible highlight; evidence saved');
      console.log(JSON.stringify(metrics,null,2));
      return;
    }

    await page.locator('.monaco-editor').waitFor({timeout:30000});
    await page.evaluate(async()=>{
      window.__monaco=(await import('/src/components/protocols/monacoXml.ts')).monaco;
      window.__cursorEvents=[];
      const editor=window.__monaco.editor.getEditors()[0];
      editor.onDidChangeCursorPosition(event=>window.__cursorEvents.push({kind:'cursor',time:performance.now(),position:event.position,reason:event.reason,source:event.source}));
      editor.onDidChangeModelContent(event=>window.__cursorEvents.push({kind:'content',time:performance.now(),isFlush:event.isFlush,length:editor.getValue().length}));
    });
    const model=async()=>{
      await page.waitForFunction(()=>window.__xmlChecks.at(-1)?.check!=null);
      return page.evaluate(()=>window.__monaco.editor.getModels().find(model=>model.getLanguageId()==='xml')?.getValue());
    };
    const goTo=async(line,column=1)=>{
      await page.locator('.monaco-editor').first().click();
      await page.keyboard.press('Control+g');
      const input=page.locator('.quick-input-widget input');
      await input.fill(':'+line+':'+column);
      await page.keyboard.press('Enter');
      await input.waitFor({state:'hidden'});
      await page.waitForFunction(()=>window.__monaco.editor.getEditors().some(editor=>editor.hasTextFocus()));
      await page.waitForFunction(()=>document.activeElement?.classList.contains('inputarea'));
      await page.waitForFunction(({line,column})=>{
        const position=window.__monaco.editor.getEditors().find(editor=>editor.getModel()?.getLanguageId()==='xml')?.getPosition();
        return position?.lineNumber===line&&position.column===column;
      },{line,column});
      await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
    };
    const load=async name=>{
      const expected=name==='载入长XML'?longXml:name==='载入空节点XML'?'<root>\n\n</root>':'<protocols>\n  <protocol name="无人机" />\n</protocols>';
      await page.getByRole('button',{name}).click();
      await page.waitForFunction(value=>window.__monaco.editor.getModels().find(model=>model.getLanguageId()==='xml')?.getValue()===value&&window.__xmlValue===value,expected);
    };
    // Rendering must retain incomplete input, including a lone opening bracket.
    await goTo(1);
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Enter');
    await page.keyboard.insertText('<');
    assert.match(await model(),/\n<>?$/);
    assert.ok((await page.locator('.view-lines').innerText()).includes('<'));
    pass('standalone opening bracket remains in the document and visible editor');

    await load('载入空节点XML');
    await goTo(2);
    await page.keyboard.press('Tab');
    await page.keyboard.type('hello');
    const indented=await model();
    assert.match(indented,/^<root>\n +hello\n<\/root>$/,'typing immediately after Tab must remain on the intended line');
    await page.keyboard.press('Control+z');
    assert.doesNotMatch(await model(),/hello/);
    await page.keyboard.press('Control+y');
    assert.equal(await model(),indented);
    pass('Tab then immediate typing retains caret; undo/redo restores the input group');

    await load('载入长XML');
    await goTo(780);
    await page.keyboard.press('Home');
    await page.keyboard.insertText('<!--第780行-->');
    let source=await model();
    assert.match(source.split('\n')[779],/^\s*<!--第780行-->/);
    assert.ok(source.split('\n')[779].includes('p778'));
    await page.keyboard.press('End');
    await page.keyboard.insertText('<!--横向末尾-->');
    source=await model();
    assert.match(source.split('\n')[779],/<!--横向末尾-->$/);
    const position=await page.evaluate(()=>{
      const editor=window.__monaco.editor.getEditors().find(editor=>editor.getModel()?.getLanguageId()==='xml');
      return {position:editor.getPosition(),scrollLeft:editor.getScrollLeft()};
    });
    assert.equal(position.position.lineNumber,780);
    assert.equal(position.position.column,source.split('\n')[779].length+1);
    assert.ok(position.scrollLeft>0,'long line horizontally scrolls to the editing caret');
    const rendered=await page.locator('.view-lines > .view-line').count();
    assert.ok(rendered>0&&rendered<150,`1004-line document renders ${rendered} visible lines`);
    await page.screenshot({path:path.join(out,'long-line-780.png'),fullPage:true});
    pass('line 780, long horizontal line and viewport-only rendering preserve character positions');

    await load('载入短XML');
    await goTo(2,18);
    const beforeIme=await model();
    const cdp=await page.context().newCDPSession(page);
    await cdp.send('Input.imeSetComposition',{text:'中文输入',selectionStart:4,selectionEnd:4});
    await cdp.send('Input.insertText',{text:'中文输入'});
    const withIme=await model();
    assert.equal(withIme.split('中文输入').length-1,1);
    assert.equal(withIme.length,beforeIme.length+4);
    await page.keyboard.press('Control+z');
    await page.waitForFunction(value=>window.__monaco.editor.getModels().find(model=>model.getLanguageId()==='xml')?.getValue()===value,beforeIme);
    await page.keyboard.press('Control+y');
    assert.equal(await model(),withIme);
    await cdp.detach();
    pass('Chinese IME composition commits once and supports undo/redo');

    await load('载入短XML');
    await goTo(1);
    await page.keyboard.press('Control+h');
    const findWidget=page.locator('.find-widget');
    await findWidget.waitFor();
    const inputs=findWidget.locator('textarea.input');
    await inputs.nth(0).fill('无人机');
    await inputs.nth(1).fill('飞行器');
    // Monaco exposes the replace-all command as a labelled action, not a document mutation.
    await findWidget.getByRole('button',{name:/Replace All|全部替换/}).click();
    await page.keyboard.press('Escape');
    assert.match(await model(),/飞行器/);
    assert.doesNotMatch(await model(),/无人机/);
    await goTo(1);
    await page.keyboard.press('Control+Shift+[');
    await page.waitForFunction(()=>Boolean(document.querySelector('.folded-background,.codicon-folding-collapsed')));
    await page.keyboard.press('Control+Shift+]');
    await page.waitForFunction(()=>!document.querySelector('.folded-background,.codicon-folding-collapsed'));
    pass('find/replace and keyboard folding/unfolding work');

    // Validation must settle after typing and expose markers for malformed XML.
    await load('载入短XML');
    await goTo(1);
    await page.evaluate(()=>{window.__keyTrace=[];document.addEventListener('keydown',event=>window.__keyTrace.push({key:event.key,code:event.code,keyCode:event.keyCode,isComposing:event.isComposing,ctrl:event.ctrlKey,shift:event.shiftKey,target:event.target.className}),{capture:true});});
    await page.keyboard.press('Control+End');
    await page.waitForFunction(()=>{
      const editor=window.__monaco.editor.getEditors()[0];
      return editor.getPosition().lineNumber===editor.getModel().getLineCount();
    });
    const sequenceEvidence=await page.evaluate(()=>{
      const editor=window.__monaco.editor.getEditors()[0];
      return {position:editor.getPosition(),keyEvents:window.__keyTrace,cursorEvents:window.__cursorEvents.slice(-20),model:{lines:editor.getModel().getLineCount(),length:editor.getValue().length}};
    });
    fs.writeFileSync(path.join(out,'shortcut-sequence-probe.json'),JSON.stringify(sequenceEvidence,null,2));
    if(sequenceEvidence.position.lineNumber!==sequenceEvidence.model.lines) failures.push('Ctrl+End after replace/fold/unfold/goto-line did not move the caret');
    // Separate lifecycle for syntax validation so a preceding shortcut failure cannot hide it.
    await page.reload();
    await page.locator('.monaco-editor').waitFor();
    await page.evaluate(async()=>{window.__monaco=(await import('/src/components/protocols/monacoXml.ts')).monaco;});
    await page.locator('.view-lines').click();
    await page.keyboard.press('Control+End');
    await page.keyboard.insertText('<');
    await page.waitForFunction(()=>window.__xmlChecks.at(-1)?.check?.ok===false);
    const validationDelay=await page.evaluate(()=>{
      const finished=window.__xmlChecks.at(-1);
      const pending=window.__xmlChecks.findLast(event=>event.check===null);
      return finished.time-pending.time;
    });
    assert.ok(validationDelay>=250,`validation waits for typing to settle (${validationDelay.toFixed(1)} ms)`);
    await page.waitForFunction(()=>window.__monaco.editor.getModelMarkers({}).some(marker=>marker.severity===8));
    const markers=await page.evaluate(()=>window.__monaco.editor.getModelMarkers({}).map(({message,startLineNumber,startColumn})=>({message,startLineNumber,startColumn})));
    assert.ok(markers[0].startLineNumber>0&&markers[0].startColumn>0);
    await page.getByRole('button',{name:'定位错误',exact:true}).click();
    const errorPosition=await page.evaluate(()=>window.__monaco.editor.getEditors().find(editor=>editor.getModel()?.getLanguageId()==='xml')?.getPosition());
    assert.equal(errorPosition.lineNumber,markers[0].startLineNumber);
    await page.screenshot({path:path.join(out,'invalid-xml-marker.png'),fullPage:true});
    await page.keyboard.press('Control+z');
    await page.waitForFunction(()=>window.__xmlChecks.at(-1)?.check?.ok===true);
    assert.equal(await page.evaluate(()=>window.__monaco.editor.getModelMarkers({}).length),0);
    pass('settled validation updates, invalid XML has positioned marker, correction clears it');

    for(const theme of ['light','dark']){
      await page.evaluate(dark=>document.documentElement.classList.toggle('dark',dark),theme==='dark');
      await page.waitForFunction(dark=>document.querySelector('.monaco-editor')?.classList.contains('vs-dark')===dark,theme==='dark');
      await page.waitForFunction(()=>Array.from(document.querySelectorAll('[role="toolbar"] button')).every(element=>element.getAnimations().every(animation=>animation.playState!=='running')));
      await page.screenshot({path:path.join(out,theme+'-xml-editor.png'),fullPage:true});
    }
    await page.setViewportSize({width:720,height:540});
    await load('载入长XML');
    await goTo(780);
    await page.screenshot({path:path.join(out,'narrow-xml-editor.png'),fullPage:true});
    const overflow=await page.evaluate(()=>document.documentElement.scrollWidth>window.innerWidth);
    assert.equal(overflow,false);
    pass('light/dark themes and narrow layout remain contained');
    assert.deepEqual(errors,[]);
    fs.writeFileSync(path.join(out,'results.json'),JSON.stringify({passed:results,validationDelayMs:validationDelay,failures,pageErrors:errors},null,2));
    assert.deepEqual(failures,[]);
    console.log('PASS: '+results.length+' XML editor scenarios');
  }catch(error){
    console.error('Browser errors:',errors);
    console.error('Page:',(await page.locator('body').innerText()).slice(-2000));
    console.error('Focus:',await page.evaluate(()=>({active:document.activeElement?.outerHTML,editors:window.__monaco?.editor.getEditors().map(editor=>({focus:editor.hasTextFocus(),position:editor.getPosition(),readOnly:editor.getOption(window.__monaco.editor.EditorOption.readOnly)}))})));
    await page.screenshot({path:path.join(out,'failure.png'),fullPage:true});
    throw error;
  }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
