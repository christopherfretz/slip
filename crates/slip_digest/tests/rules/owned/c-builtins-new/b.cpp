namespace acme { extern "C" int c_fn(int); struct Mine { int v; }; }
int use() { auto *m = new acme::Mine; int r = acme::c_fn(__builtin_abs(m->v)); delete m; return r; }
