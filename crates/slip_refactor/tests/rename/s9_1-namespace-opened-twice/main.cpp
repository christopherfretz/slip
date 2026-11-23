namespace n { void f(int); }
namespace n { void f(int) {} void f(double); }
void h() { n::f(1); }
