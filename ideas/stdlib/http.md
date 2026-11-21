# std/http

Requests to web servers. `use std/http;`, then `http::get($url)`. It's written in Lakescript, in `crates/lakescript/src/stdlib/http.lks`, with its Rust part in `http.rs` *(see How modules are made in [README.md](README.md))*. Requests are sent with the `ureq` crate, with HTTPS through `rustls`.

```
use std/http;

const $response = http::get('https://api.github.com/repos/rust-lang/rust');
if ($response.ok?()) {
    outln($response.json()['stargazers_count']);
} else {
    errln(f"GitHub said {$response.status}");
}

http::post('https://example.com/api/notes', {'title' => 'hello'});
http::download('https://example.com/file.tar.gz', 'file.tar.gz');
```

## Functions

Every function takes `$headers`, sent with the request *(i.e. `headers: {'Authorization' => f"Bearer {$token}"}`)*, and `$timeout`, the seconds the whole request can take, or `null` to wait forever.

| Function | Returns | Description |
|----------|---------|-------------|
| `get(string $url, map<string, string> $headers = {}, ?number $timeout = 30)` | `Response` | Gets the page or the data at `$url` |
| `post(string $url, any $body = null, map<string, string> $headers = {}, ?number $timeout = 30)` | `Response` | Sends `$body` to `$url` |
| `put(string $url, any $body = null, map<string, string> $headers = {}, ?number $timeout = 30)` | `Response` | The same with the method `PUT` |
| `patch(string $url, any $body = null, map<string, string> $headers = {}, ?number $timeout = 30)` | `Response` | The same with the method `PATCH` |
| `delete(string $url, map<string, string> $headers = {}, ?number $timeout = 30)` | `Response` | A request with the method `DELETE` |
| `request(string $method, string $url, any $body = null, map<string, string> $headers = {}, ?number $timeout = 30)` | `Response` | A request with any method *(i.e. `http::request('HEAD', $url)`)* |
| `download(string $url, string $path, map<string, string> $headers = {}, ?number $timeout = 300)` | `Response` | Writes what's at `$url` to the file `$path`, without keeping it in memory, if the status is from 200 to 299. The response's body is then empty. With another status, the file isn't touched and the body is the server's answer |

- **The body:** a string is sent as it is. Any other value is sent as JSON, with the header `Content-Type: application/json` unless `$headers` gives one. `null` sends nothing.
- **Redirects** are followed, up to 10.
- Every request says it comes from `lakescript/<version>`, in the header `User-Agent`. `$headers` can give another.

## Response

What a server answered.

| Field | Type | Description |
|-------|------|-------------|
| `status` | `number` | The status code *(i.e. `200` or `404`)* |
| `headers` | `map<string, string>` | The headers, with their names in lowercase. A header that came several times has its values joined with `, ` |
| `body` | `string` | The body as text. Bytes that aren't valid UTF-8 become `�`. Empty after `download()` |
| `url` | `string` | The address that answered, after following redirects |

| Method | Returns | Description |
|--------|---------|-------------|
| `ok?()` | `bool` | `true` for a status from 200 to 299 |
| `json()` | `any` | The body read as JSON. Throws a `ValueError` if it isn't *(see [format/json.md](format/json.md))* |
| `header(string $name)` | `?string` | A header by its name, in any case, or `null` *(i.e. `$response.header('Content-Type')`)* |

## Errors

- **A status of 4xx or 5xx isn't an error.** It's an answer like any other, and the script checks it with `ok?()` or `status`. Scripts often expect some of them *(i.e. a 404 when asking if something exists)*, and a `try` around every request would hide the ones that matter.
- **No answer at all** throws a `NetworkError`, under `IOError`: no network, a name that doesn't resolve, a refused connection, a timeout, too many redirects. Its message says which *(i.e. `can't reach 'http://localhost:9999/': the server refused the connection`)*.
- An address that isn't valid throws a `ValueError`, and so does a timeout that isn't a positive number.

## Backlog

- Forms *(i.e. `form: {'name' => 'Ana'}`)* and uploading files.
- Query parameters as a map *(i.e. `query: {'q' => 'lake script'}`)*, with their characters escaped.
- Binary bodies, once there's a type for bytes.
- Sessions that keep cookies and connections between requests.
