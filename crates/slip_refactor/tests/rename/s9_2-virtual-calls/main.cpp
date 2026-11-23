namespace n {
struct B { virtual void f(); };
struct D : B {
  void f() override;
  void k() { this->f(); B::f(); f(); }
};
}
void h(n::B* p, n::D& d) {
  p->f();
  d.n::B::f();
  void (n::B::*m)() = &n::B::f;
  (d.*m)();
  void (n::D::*md)() = &n::D::f;
  (d.*md)();
}
