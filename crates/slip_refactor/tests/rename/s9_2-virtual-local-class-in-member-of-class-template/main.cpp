namespace n { struct B { virtual void f(); }; }
template <class T> struct S { int t() { struct L : n::B { void f() override {} }; L l; l.f(); return 0; } };
int u = S<int>().t() + S<long>().t();
