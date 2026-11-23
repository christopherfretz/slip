namespace n { struct { int a; } *p; }
int h() { return n::p->a; }
