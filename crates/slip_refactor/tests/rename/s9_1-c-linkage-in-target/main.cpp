namespace n { extern "C" void f(int); }
void h() { n::f(1); }
