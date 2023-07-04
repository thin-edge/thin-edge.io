---
title: Markdown guidelines
tags: [Documentation]
sidebar_position: 2
---

A guideline to writing markdown in a consistent manner.

## Headers

### Avoid in-line code blocks in headers

Headers should not include in-line code blocks.

**✅ Good**

````md
## thin-edge.io info
````

**❌ Bad**

````md
## `thin-edge.io` info
````

### Don't end a subsection with a colon

Headers (either headers or bold text acting as a header) does not need punctuation.

**✅ Good**

````md
**Topic**
````

**❌ Bad**

````md
**Topic:**
````


## Code blocks

### Shell/console syntax highlighting

Use the `sh` syntax highlight option for any console/terminal related code blocks.

**✅ Good**

````md
```sh
tedge --help
```
````

**❌ Bad**

````md
```console
tedge --help
```
````

````md
```shell
tedge --help
```
````

### Console output

Console output should use the `text` syntax highlighter and should have a title block indicating that it is output.

You can use other syntax highlighting if the output is of another format (e.g. json).

**✅ Good**

````md
```sh title="Output"
tedge --help
```
````

**❌ Bad**

````md
```sh
tedge --help
```
````

**✅ Good**

````md
```json title="Output"
{
   "text": "example"
}
```
````

**❌ Bad: Incorrect syntax highlighting for the output**

````md
```sh
{
   "text": "example"
}
```
````

### Avoid in-line code blocks for long commands

In-line code blocks are not easy for users to copy the text from. Use a full code-block instead as these blocks are rendered with a "copy" button built-in.

**✅ Good**

````md
**Topic**
   
```text
tedge/{child-d}/commands/res/config_snapshot
```
````

**❌ Bad**

````md
**Topic**
   
   `tedge/{child-d}/commands/res/config_snapshot`
````

### Systemd/journald service commands

Don't use the `.service` suffix in calls to `systemctl` and `journalctl`.

**✅ Good**

````md
```sh
sudo systemctl restart tedge-mapper-c8y
```
````

**❌ Bad**

````md
```sh
sudo systemctl restart tedge-mapper-c8y.service
```
````

### Set file path in title

If the contents of a code block relate to the contents of a file, then the file path should be included in the title.

**✅ Good**

````md
```toml title="file: /etc/tedge/tedge.toml"
[c8y]
url = "mytenant.cumulocity.com"

[mqtt]
bind_address = "127.0.0.1"
```
````

**❌ Bad**

````md
```toml
[c8y]
url = "mytenant.cumulocity.com"

[mqtt]
bind_address = "127.0.0.1"
```
````
