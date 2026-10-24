# Web and document primitives

FastLang's standard library includes document values, HTML construction, HTTP servers, and FastReact-oriented response helpers.

## HTML documents

```fast
str name = "FastLang";
HtmlDocument page = @html {
    <main>
        <h1>Hello, @f{name}</h1>
    </main>
};
```

`@f{...}` embeds a FastLang expression into the document tree.

## HTTP routes

```fast
Server app = new Server();

app.get("/", fn(_req: HttpRequest, res: HttpResponse) -> void {
    res.send_html(@html { <h1>Hello</h1> });
});

app.listen(() -> { println("server started"); }, 8080);
```

## Reactive attributes

FastReact-compatible attributes can describe client-triggered requests and targeted updates:

```fast
<button onclick="/increment" target="#counter" swap="outerHTML">
    Increment
</button>
```

The web layer is a standard-library/runtime concern. The core language only defines the syntax needed to construct and pass document values.

