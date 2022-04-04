module.exports.scroll = function scroll() {
	var body = document.body;
	var html = document.documentElement;
	var height = Math.max(
		body.scrollHeight,
		body.offsetHeight,
		html.clientHeight,
		html.scrollHeight,
		html.offsetHeight
	);
	var width = Math.max(
		body.scrollWidth,
		body.offsetWidth,

		html.clientWidth,
		html.scrollWidth,
		html.offsetWidth
	);
	var container = document.getElementsByClassName("container")[0];
	container.style.height = height + "px";
	container.style.width = width + "px";
    console.log("Loaded")
}
