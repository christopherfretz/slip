namespace n { void f(int); void f(const char*); }
#define BOTH(x) x(1); x("s")
void h() { BOTH(n::f); }
