namespace n { struct C { template <class U> void k(U); void f(int); }; }
void h() { n::C c; c.f(1); }
