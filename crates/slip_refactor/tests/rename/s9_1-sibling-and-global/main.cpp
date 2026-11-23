namespace n { void f(int); }
namespace m { void f(int); }
void f(int);
void h() { n::f(1); m::f(2); f(3); ::f(4); }
