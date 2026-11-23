namespace n { struct B { void f(int); }; struct D : B { void g(long); void k() { f(1); } }; }
void h() { n::D d; d.f(1); }
