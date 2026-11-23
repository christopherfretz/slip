namespace legacy { extern "C" int c_fn(int); struct Mine { int v; }; }
int use() { auto *m = new legacy::Mine; int r = legacy::c_fn(__builtin_abs(m->v)); delete m; return r; }
