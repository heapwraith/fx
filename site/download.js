// Real data from GitHub, filled in when it arrives; without it the page keeps
// its static links. The newest published release (prereleases included:
// GitHub's "latest" skips them while Pathdash is 0.x) gives the installer
// link, version and size; the repository gives the star count.
(function () {
  document.documentElement.classList.add("js");
  var en = document.documentElement.lang === "en";
  var arm = /arm|aarch64/i.test(navigator.userAgent + " " + (navigator.platform || ""));
  var api = "https://api.github.com/repos/heapwraith/fx";
  var json = function (url) {
    return fetch(url, { headers: { Accept: "application/vnd.github+json" } })
      .then(function (r) { return r.ok ? r.json() : Promise.reject(r.status); });
  };
  var num = function (n, digits) {
    return n.toLocaleString(en ? "en-US" : "pt-BR", { maximumFractionDigits: digits || 0 });
  };

  json(api + "/releases?per_page=10").then(function (releases) {
    var rel = releases.find(function (r) { return !r.draft; });
    if (!rel) return;
    var want = arm ? "-setup-arm64.exe" : "-setup-x64.exe";
    var asset = rel.assets.find(function (a) { return a.name.endsWith(want); });
    var version = rel.tag_name.replace(/^v/, "");
    document.querySelectorAll("[data-version]").forEach(function (el) {
      el.textContent = el.getAttribute("data-version")
        .replace("{v}", version)
        .replace("{arch}", arm ? "ARM64" : "x64")
        .replace("{mb}", asset ? num(asset.size / 1048576, 1) : "?");
      el.hidden = false;
    });
    document.querySelectorAll("[data-release]").forEach(function (a) { a.href = rel.html_url; });
    if (!asset) return;
    document.querySelectorAll("[data-download]").forEach(function (a) { a.href = asset.browser_download_url; });
  }).catch(function () {});

  json(api).then(function (repo) {
    // A handful of stars says less than none: shown from 10 up.
    if (repo.stargazers_count < 10) return;
    document.querySelectorAll("[data-stars]").forEach(function (el) {
      el.textContent = "★ " + num(repo.stargazers_count);
      el.hidden = false;
    });
  }).catch(function () {});

  // Sections ease in once, as they come into view.
  if ("IntersectionObserver" in window) {
    var seen = new IntersectionObserver(function (entries) {
      entries.forEach(function (e) {
        if (e.isIntersecting) { e.target.classList.add("in"); seen.unobserve(e.target); }
      });
    }, { rootMargin: "0px 0px -10% 0px" });
    document.querySelectorAll(".reveal").forEach(function (el) { seen.observe(el); });
  } else {
    document.querySelectorAll(".reveal").forEach(function (el) { el.classList.add("in"); });
  }
})();
