struct S { int x; S(); ~S(); };
S make();
int use(S *p) { return p->x; }
