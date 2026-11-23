namespace n { extern "C++" { void g(long); } void f(int); }
void h() { n::f(1); n::g(2); }
