# **Stress Test Examples**

---

### **1. Inline suppression and overlapping delimiters**

```markdown
*italic **bold inside italic***  <- bold closes inside italic
**bold *italic inside bold***   <- italic closes inside bold
*italic $math * still math$*    <- unmatched * inside math
`inline $math$ code`            <- math inside inline code
```

* Checks:

  * Suppression of inline parsing inside math/code.
  * Compile errors on unmatched delimiters.
  * Longest delimiter wins (bold vs italic).

---

### **2. Nested list with mixed blocks**

````markdown
- first item
    -[a] nested item
        ```python
        def f(x):
            return x
        ```
        $$\int_0^1 x dx$$
    -[i] another nested
````

* Checks:

  * Indentation consistency across nested lists and blocks.
  * Code and math blocks respecting list indentation.
  * Multilevel list parsing.

---

### **3. Paragraph + line breaks + `\n` macro**

```markdown
This is a paragraph with a line break\n
And continues on the same paragraph.

This is a new paragraph.
```

* Checks:

  * `\n` -> `<br>` without ending paragraph.
  * Blank line triggers paragraph boundary.

---

### **4. Macros introducing blocks**

````markdown
\macro1  <- expands to "- new list item\n    ```js\nconsole.log('hi')\n```"
\macro2  <- expands to "\$x^2\$" inside paragraph

Normal text
````

* Checks:

  * Macro expansion can generate blocks/code/math.
  * Expansion occurs **before parsing**.
  * Macro inside math suppressed.
  * Ordering is top-to-bottom, no forward references.

---

### **5. Superscript/subscript edge cases**

```markdown
x^{2 + y_{i}}  <- invalid: nested formatting inside sub/sup
x^{2 + \{y\}} <- valid: escaped braces
```

* Checks:

  * Braces must be balanced.
  * Formatting inside sub/sup prohibited.
  * Escaped braces allowed.

---

### **6. Footnotes with tricky URLs**

```markdown
Here is a citation^[ref](https://example.com/a(b)c)
Another one^[ref](https://example.com/x))
```

* Checks:

  * URLs containing `)` preserved correctly.
  * Auto-numbering remains correct.

---

### **7. Display math with indentation + list integration**

```markdown
- list item
    $$\sum_{i=1}^{n} i$$
- next item
```

* Checks:

  * Display math respects indentation of enclosing list.
  * Inline parsing suppressed inside math.
  * Paragraphs inside list items remain consistent.

---

### **8. Conflicting macro expansions**

```markdown
\macroA <- expands to "\macroB"
\macroB <- expands to "\macroA"
```

* Checks:

  * Infinite recursion detection.
  * Compile error if recursion loop exists.

---

### **9. Edge case headings and cross-references**

```markdown
#[1] Introduction {#intro}
#[2] Overview {#overview}
See #[1] and #[2]
```

* Checks:

  * Optional explicit IDs are respected.
  * Cross-references resolve correctly.

---

### **10. Inline + code + math + macro mixed**

```markdown
*Start \macro1 with `inline code $x^2$` and $y^2$* 
```

* Checks:

  * Inline suppression of code/math.
  * Macro expansion before parsing.
  * Compile error if unmatched delimiters.
