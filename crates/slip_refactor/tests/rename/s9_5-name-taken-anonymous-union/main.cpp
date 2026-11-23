namespace n { static union { int g; }; void f(int); }
void h() { n::f(1); }
