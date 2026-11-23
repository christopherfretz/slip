namespace legacy { typedef struct { int a; } P; P make(); }
int use() { return legacy::make().a; }
