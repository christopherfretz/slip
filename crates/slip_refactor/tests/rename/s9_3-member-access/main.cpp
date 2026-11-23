namespace n {
struct C {
  void f();
  int x;
};
void C::f() {}
}
void h(n::C a, n::C* p) {
  a.f();
  p->f();
  a.C::f();
  a.n::C::f();
}
