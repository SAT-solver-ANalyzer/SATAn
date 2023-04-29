final: prev: {
  duckdb = prev.duckdb.overridePythonAttrs
    (old: {
      postPatch = ''
        cd tools/pythonpkg

        substituteInPlace setup.py \
          --replace 'multiprocessing.cpu_count()' "$NIX_BUILD_CORES" \
          --replace 'setuptools_scm<7.0.0' 'setuptools_scm'
      '';
    });
}

