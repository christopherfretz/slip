struct T { int x; T(); ~T(); };
T make();
int use(T *p) { return p->x; }
