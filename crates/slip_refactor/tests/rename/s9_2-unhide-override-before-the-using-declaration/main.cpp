namespace n {
struct B { virtual void f(); void f(int); };
struct D : B { void f() override; using B::f; };
}
void h(n::D& d) { d.f(); d.f(1); }
