namespace n { typedef struct { int a; } S; S v; }
int h() { return n::v.a; }
