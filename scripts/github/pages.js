const ghpages = require("gh-pages");

ghpages.publish("./static", {
    branch: "website",
    repo: "https://github.com/phoenix-language/phoenix.git",
    message: "Website updated"
}, function (err) {
    if (err) {
        console.log(err);
    } else {
        console.log("Website published to github pages!");
    }
});