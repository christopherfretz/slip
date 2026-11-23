void f(int);
namespace n { void f(int); }
void h() { f(1); ::f(2); n::f(3); }
