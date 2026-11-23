namespace n { void f(int); }
namespace m { using n::f; }
void h() { m::f(1); }
