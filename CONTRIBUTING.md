# Contributing

`cargo-sweep` is a community tool with a single maintainer. I encourage pull requests and bug
reports, but for my sanity, I will be opinionated about feature requests.

To get started:
1. Find an issue to work on
1. Add a test to make sure it doesn't work today. It's important to do this before writing code so you can be sure your test actually tests something.
1. Fix the problem and run the tests.
1. Make a PR.

You may also want to set up the pre-push hook: `ln -s ../../ci/pre-push .git/hooks`
