namespace n { void f(int); void f(double); }
namespace m { using n::f; }
void h() { m::f(1); m::f(2.0); }
