// Pan and zoom for the module map. The layout itself is computed by the
// engine and baked into the SVG, so this only moves the viewport: reopening
// the report always shows the same picture.
(function () {
  var svg = document.querySelector("svg[data-map]");
  if (!svg) {
    return;
  }
  var group = svg.querySelector("g");
  var view = { x: 0, y: 0, scale: 1 };
  var drag = null;

  function apply() {
    group.setAttribute(
      "transform",
      "translate(" + view.x + " " + view.y + ") scale(" + view.scale + ")"
    );
  }

  svg.addEventListener("wheel", function (event) {
    event.preventDefault();
    var box = svg.getBoundingClientRect();
    var ratio = svg.viewBox.baseVal.width / box.width;
    var px = (event.clientX - box.left) * ratio;
    var py = (event.clientY - box.top) * ratio;
    var step = event.deltaY < 0 ? 1.12 : 1 / 1.12;
    var next = Math.min(6, Math.max(0.4, view.scale * step));
    view.x = px - ((px - view.x) * next) / view.scale;
    view.y = py - ((py - view.y) * next) / view.scale;
    view.scale = next;
    apply();
  });

  svg.addEventListener("pointerdown", function (event) {
    drag = { x: event.clientX, y: event.clientY, ox: view.x, oy: view.y };
    svg.classList.add("dragging");
    svg.setPointerCapture(event.pointerId);
  });

  svg.addEventListener("pointermove", function (event) {
    if (!drag) {
      return;
    }
    var box = svg.getBoundingClientRect();
    var ratio = svg.viewBox.baseVal.width / box.width;
    view.x = drag.ox + (event.clientX - drag.x) * ratio;
    view.y = drag.oy + (event.clientY - drag.y) * ratio;
    apply();
  });

  function release() {
    drag = null;
    svg.classList.remove("dragging");
  }

  svg.addEventListener("pointerup", release);
  svg.addEventListener("pointercancel", release);
})();
