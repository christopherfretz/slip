namespace n { void f(int); }
void h() { using n::f; f(1); }
