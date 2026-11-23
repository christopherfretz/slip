namespace n { void f(int); }
namespace m = n;
void h() { m::f(1); }
