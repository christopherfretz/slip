namespace n { struct B { virtual void f(); }; }
void k() {
  struct L : n::B { void f() override {} };
  L l;
  l.f();
}
