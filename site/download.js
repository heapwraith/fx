// Points the download buttons at the installer of the newest published
// release (prereleases included: GitHub's "latest" skips them while
// Pathdash is 0.x). Without the API the buttons keep the Releases page.
(function () {
  var api = "https://api.github.com/repos/heapwraith/fx/releases?per_page=10";
  var arm = /arm|aarch64/i.test(navigator.userAgent + " " + (navigator.platform || ""));
  fetch(api, { headers: { Accept: "application/vnd.github+json" } })
    .then(function (r) { return r.ok ? r.json() : Promise.reject(r.status); })
    .then(function (releases) {
      var rel = releases.find(function (r) { return !r.draft; });
      if (!rel) return;
      var want = arm ? "-setup-arm64.exe" : "-setup-x64.exe";
      var asset = rel.assets.find(function (a) { return a.name.endsWith(want); });
      if (!asset) return;
      var version = rel.tag_name.replace(/^v/, "");
      document.querySelectorAll("[data-download]").forEach(function (a) {
        a.href = asset.browser_download_url;
      });
      document.querySelectorAll("[data-version]").forEach(function (el) {
        el.textContent = el.getAttribute("data-version")
          .replace("{v}", version)
          .replace("{arch}", arm ? "ARM64" : "x64")
          .replace("{mb}", (asset.size / 1048576).toFixed(1).replace(".", el.lang === "en" ? "." : ","));
        el.hidden = false;
      });
    })
    .catch(function () {});
})();
