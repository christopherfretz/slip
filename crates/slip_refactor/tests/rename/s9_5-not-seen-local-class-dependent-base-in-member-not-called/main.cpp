namespace n { struct B { virtual void f(); }; }
template <class T> struct S { int t() { struct L : T { void f() override {} }; L l; l.f(); return 0; } };
S<n::B> s;
