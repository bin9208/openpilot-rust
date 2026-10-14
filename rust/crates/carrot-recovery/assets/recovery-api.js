"use strict";
(function () {
  async function requestJson(url, options) {
    var response = await fetch(url, options || {});
    var text = await response.text();
    var payload = {};

    if (text) {
      try {
        payload = JSON.parse(text);
      } catch (err) {
        throw new Error(text.slice(0, 500) || ("HTTP " + response.status));
      }
    }

    if (!response.ok || (payload && payload.ok === false)) {
      var error = new Error((payload && (payload.error || payload.out)) || ("HTTP " + response.status));
      error.status = response.status;
      error.payload = payload || null;
      throw error;
    }
    return payload;
  }

  function getJson(url) {
    return requestJson(url, { cache: "no-store" });
  }

  function postJson(url, body) {
    return requestJson(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body || {}),
    });
  }

  var api = Object.freeze({ requestJson: requestJson, getJson: getJson, postJson: postJson });
  window.CarrotRecoveryApi = api;

  // Compatibility contract for the shared support_terminal.js component.
  // These globals exist only in the recovery document and are backed by the
  // recovery-owned, same-origin transport above.
  window.getJson = getJson;
  window.postJson = postJson;
})();
