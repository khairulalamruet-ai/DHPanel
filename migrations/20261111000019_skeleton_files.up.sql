-- T04.14: the default files every new tenant starts with (its docroot gets a
-- copy when its directories are first made). Administrators edit the list;
-- changing it never touches tenants that already exist.
--
-- Text only and small (see crates/node-protocol): the files travel inside the
-- set-up operation, which is one bounded message from the control plane to the
-- node and on to the privileged helper.
CREATE TABLE skeleton_files (
    path        text        PRIMARY KEY
        CONSTRAINT skeleton_files_path_format CHECK (
            length(path) BETWEEN 1 AND 450
            AND path ~ '^[A-Za-z0-9._-]{1,64}(/[A-Za-z0-9._-]{1,64}){0,5}$'
            AND path !~ '(^|/)\.{1,2}(/|$)'
        ),
    content     text        NOT NULL DEFAULT ''
        CONSTRAINT skeleton_files_content_size CHECK (octet_length(content) <= 16384),
    updated_by  uuid        REFERENCES accounts (id) ON DELETE SET NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TRIGGER skeleton_files_set_updated_at
    BEFORE UPDATE ON skeleton_files
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

INSERT INTO skeleton_files (path, content) VALUES ('index.html', $html$<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Your site is ready</title>
  <style>
    body { font-family: system-ui, sans-serif; margin: 0; min-height: 100vh; display: grid; place-items: center; background: #f5f6f8; color: #1d2330; }
    main { max-width: 32rem; padding: 2rem; text-align: center; }
    h1 { margin: 0 0 .5rem; font-size: 1.75rem; }
    p { margin: 0; color: #4a5368; line-height: 1.5; }
  </style>
</head>
<body>
  <main>
    <h1>Your site is ready</h1>
    <p>This is the default page of your hosting account. Upload your own <code>index.html</code> or <code>index.php</code> to replace it.</p>
  </main>
</body>
</html>
$html$);
