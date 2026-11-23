namespace acme { typedef struct { int a; } P; P make(); }
int use() { return acme::make().a; }
