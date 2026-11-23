namespace legacy { typedef struct { int a; } Q; Q make(); }
int use() { return legacy::make().a; }
