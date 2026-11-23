namespace n { void f(int); }
void h() { n::f(1); undeclared(); }
