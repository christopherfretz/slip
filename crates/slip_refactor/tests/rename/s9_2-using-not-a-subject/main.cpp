namespace n { void f(int); }
namespace a { void f(double); }
namespace m { using a::f; }
void h() { m::f(1.0); n::f(1); }
