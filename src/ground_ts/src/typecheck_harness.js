/// In-memory TypeScript type-checker harness.
///
/// Loaded by ground_ts::typecheck after the TypeScript UMD bundle sets `globalThis.ts`.
/// Exposes `globalThis.__groundTypecheck(declsContent, userContent, lib5Content) -> JSON string`.
///
/// Virtual filesystem layout:
///   "lib.es5.d.ts"    — ES5 standard lib (Array, Object, JSON …), included as explicit root
///   "decls.gen.d.ts"  — generated interface declarations
///   "user.ts"         — user-written hook implementations
///
/// Uses `noLib: true` so TypeScript does NOT look for any default lib by name.
/// The ES5 lib is included explicitly in `rootNames` instead, so it is available
/// without triggering file-not-found errors for `lib.esnext.full.d.ts` etc.
///
/// Note: with `noLib: true`, `/// <reference lib="..." />` directives inside lib.es5.d.ts
/// (e.g. `/// <reference lib="decorators" />`) are silently ignored by the compiler.

(function () {
    "use strict";

    function groundTypecheck(declsContent, userContent, lib5Content) {
        var ts = globalThis.ts;
        if (!ts) {
            return JSON.stringify([{ message: "TypeScript compiler not loaded", code: 0, category: 1, file: null, line: null, col: null }]);
        }

        var files = {
            "lib.es5.d.ts":   lib5Content  || "",
            "decls.gen.d.ts": declsContent || "",
            "user.ts":        userContent  || "",
        };

        var options = {
            // noLib: true — do not auto-include any default lib file.
            // We provide lib.es5.d.ts as an explicit root file below.
            noLib:  true,
            noEmit: true,
            strict: true,
            // Allow unannotated parameters: hook authors may omit type annotations
            // and still get useful type-checking on the annotated parts of their code.
            noImplicitAny: false,
            target: ts.ScriptTarget.ES2020,
            module: ts.ModuleKind.None,
        };

        var host = {
            getSourceFile: function (name, langVer) {
                var src = files[name];
                if (src !== undefined) {
                    return ts.createSourceFile(name, src, langVer, /*setParentNodes=*/true);
                }
                return undefined;
            },
            getDefaultLibFileName: function (o) { return ts.getDefaultLibFileName(o); },
            writeFile:             function () {},
            getCurrentDirectory:   function () { return ""; },
            getCanonicalFileName:  function (f) { return f; },
            useCaseSensitiveFileNames: function () { return true; },
            getNewLine:            function () { return "\n"; },
            fileExists:            function (f) { return Object.prototype.hasOwnProperty.call(files, f); },
            readFile:              function (f) { return files[f]; },
            directoryExists:       function () { return false; },
            getDirectories:        function () { return []; },
        };

        // Include lib.es5.d.ts as an explicit root so standard types (Array, String …)
        // are available. Only add non-empty files.
        var rootNames = ["lib.es5.d.ts", "decls.gen.d.ts", "user.ts"]
            .filter(function (k) { return !!files[k]; });

        var program = ts.createProgram(rootNames, options, host);
        var allDiags = ts.getPreEmitDiagnostics(program);

        var out = [];
        allDiags.forEach(function (d) {
            var line = null, col = null;
            if (d.file && d.start !== undefined && d.start !== null) {
                var lc = d.file.getLineAndCharacterOfPosition(d.start);
                line = lc.line + 1;
                col  = lc.character + 1;
            }
            out.push({
                message:  ts.flattenDiagnosticMessageText(d.messageText, "\n"),
                code:     d.code,
                category: d.category, // 1=error, 2=warning, 3=suggestion, 4=message
                file:     d.file ? d.file.fileName : null,
                line:     line,
                col:      col,
            });
        });

        return JSON.stringify(out);
    }

    globalThis.__groundTypecheck = groundTypecheck;
}());
