namespace n { void f(int); }
namespace m { void f(int); }
#define IN_BOTH(x) n::x(1); m::x(2)
void h() { IN_BOTH(f); }
