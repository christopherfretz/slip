namespace n { struct B { virtual void f(); }; }
template <class T> void k() {
  struct L : n::B { void f() override {} };
  L l;
  l.f();
}
template void k<int>();
