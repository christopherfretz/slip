namespace n { void f(int); }
#define CALL(x) x(1)
void h() { CALL(n::f); }
