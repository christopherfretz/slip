namespace n { struct B { void g(long); }; struct D; struct D : B { void f(int); }; }
void h() { n::D d; d.f(1); d.g(2); }
