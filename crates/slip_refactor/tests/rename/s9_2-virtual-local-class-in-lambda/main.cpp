namespace n { struct B { virtual void f(); }; }
void k() { auto l = [] { struct L : n::B { void f() override {} }; L x; x.f(); }; l(); }
