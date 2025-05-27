// Populate the sidebar
//
// This is a script, and not included directly in the page, to control the total size of the book.
// The TOC contains an entry for each page, so if each page includes a copy of the TOC,
// the total size of the page becomes O(n**2).
class MDBookSidebarScrollbox extends HTMLElement {
    constructor() {
        super();
    }
    connectedCallback() {
        this.innerHTML = '<ol class="chapter"><li class="chapter-item expanded "><a href="introduction.html"><strong aria-hidden="true">1.</strong> Introduction</a></li><li class="chapter-item expanded "><a href="installation.html"><strong aria-hidden="true">2.</strong> Installation</a></li><li class="chapter-item expanded "><a href="usage.html"><strong aria-hidden="true">3.</strong> Basic usage</a></li><li class="chapter-item expanded "><a href="tutorial.html"><strong aria-hidden="true">4.</strong> Tutorial</a></li><li class="chapter-item expanded "><a href="rust-src-repo.html"><strong aria-hidden="true">5.</strong> Rust source repo</a></li><li class="chapter-item expanded "><a href="boundaries.html"><strong aria-hidden="true">6.</strong> Bisection boundaries</a></li><li class="chapter-item expanded "><a href="rustup.html"><strong aria-hidden="true">7.</strong> Rustup toolchains</a></li><li class="chapter-item expanded "><a href="git-bisect.html"><strong aria-hidden="true">8.</strong> Git bisect a custom build</a></li><li class="chapter-item expanded "><a href="alt.html"><strong aria-hidden="true">9.</strong> Alt builds</a></li><li class="chapter-item expanded "><a href="examples/index.html"><strong aria-hidden="true">10.</strong> Examples</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="examples/diagnostics.html"><strong aria-hidden="true">10.1.</strong> Checking diagnostics</a></li><li class="chapter-item expanded "><a href="examples/windows-scripting.html"><strong aria-hidden="true">10.2.</strong> Scripting on Windows</a></li><li class="chapter-item expanded "><a href="examples/incremental.html"><strong aria-hidden="true">10.3.</strong> Incremental compilation</a></li><li class="chapter-item expanded "><a href="examples/slow.html"><strong aria-hidden="true">10.4.</strong> Slow or hung compilation</a></li><li class="chapter-item expanded "><a href="examples/components.html"><strong aria-hidden="true">10.5.</strong> Using extra components</a></li><li class="chapter-item expanded "><a href="examples/without-cargo.html"><strong aria-hidden="true">10.6.</strong> Running without Cargo</a></li><li class="chapter-item expanded "><a href="examples/preserve.html"><strong aria-hidden="true">10.7.</strong> Preserving toolchains</a></li><li class="chapter-item expanded "><a href="examples/rustdoc.html"><strong aria-hidden="true">10.8.</strong> Bisecting Rustdoc</a></li><li class="chapter-item expanded "><a href="examples/clippy.html"><strong aria-hidden="true">10.9.</strong> Bisecting Clippy</a></li><li class="chapter-item expanded "><a href="examples/doc-change.html"><strong aria-hidden="true">10.10.</strong> Documentation changes</a></li><li class="chapter-item expanded "><a href="examples/flaky.html"><strong aria-hidden="true">10.11.</strong> Flaky errors</a></li></ol></li></ol>';
        // Set the current, active page, and reveal it if it's hidden
        let current_page = document.location.href.toString().split("#")[0].split("?")[0];
        if (current_page.endsWith("/")) {
            current_page += "index.html";
        }
        var links = Array.prototype.slice.call(this.querySelectorAll("a"));
        var l = links.length;
        for (var i = 0; i < l; ++i) {
            var link = links[i];
            var href = link.getAttribute("href");
            if (href && !href.startsWith("#") && !/^(?:[a-z+]+:)?\/\//.test(href)) {
                link.href = path_to_root + href;
            }
            // The "index" page is supposed to alias the first chapter in the book.
            if (link.href === current_page || (i === 0 && path_to_root === "" && current_page.endsWith("/index.html"))) {
                link.classList.add("active");
                var parent = link.parentElement;
                if (parent && parent.classList.contains("chapter-item")) {
                    parent.classList.add("expanded");
                }
                while (parent) {
                    if (parent.tagName === "LI" && parent.previousElementSibling) {
                        if (parent.previousElementSibling.classList.contains("chapter-item")) {
                            parent.previousElementSibling.classList.add("expanded");
                        }
                    }
                    parent = parent.parentElement;
                }
            }
        }
        // Track and set sidebar scroll position
        this.addEventListener('click', function(e) {
            if (e.target.tagName === 'A') {
                sessionStorage.setItem('sidebar-scroll', this.scrollTop);
            }
        }, { passive: true });
        var sidebarScrollTop = sessionStorage.getItem('sidebar-scroll');
        sessionStorage.removeItem('sidebar-scroll');
        if (sidebarScrollTop) {
            // preserve sidebar scroll position when navigating via links within sidebar
            this.scrollTop = sidebarScrollTop;
        } else {
            // scroll sidebar to current active section when navigating via "next/previous chapter" buttons
            var activeSection = document.querySelector('#sidebar .active');
            if (activeSection) {
                activeSection.scrollIntoView({ block: 'center' });
            }
        }
        // Toggle buttons
        var sidebarAnchorToggles = document.querySelectorAll('#sidebar a.toggle');
        function toggleSection(ev) {
            ev.currentTarget.parentElement.classList.toggle('expanded');
        }
        Array.from(sidebarAnchorToggles).forEach(function (el) {
            el.addEventListener('click', toggleSection);
        });
    }
}
window.customElements.define("mdbook-sidebar-scrollbox", MDBookSidebarScrollbox);
