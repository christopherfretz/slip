namespace n { struct B { virtual void f(); }; }
template <class T> int v = [] { struct L : n::B { void f() override {} }; L l; l.f(); return 0; }();
int x = v<int> + v<long>;
