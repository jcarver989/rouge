pub(crate) const BRIDGE_NAME: &str = "rouge";

pub(crate) const BRIDGE_SCRIPT: &str = "(function(){\
if(window.__rougeBridge)return;window.__rougeBridge=true;\
window.addEventListener('message',function(event){\
if(event.source!==window)return;\
var data=event.data;\
window.webkit.messageHandlers.rouge.postMessage(typeof data==='string'?data:JSON.stringify(data));\
});})();";
