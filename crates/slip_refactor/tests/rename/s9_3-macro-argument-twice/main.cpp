namespace n { void f(int); }
#define TWICE(x) x(1); x(2)
void h() { TWICE(n::f); }
