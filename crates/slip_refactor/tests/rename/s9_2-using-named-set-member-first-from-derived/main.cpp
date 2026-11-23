namespace n {
struct A { void f(int); };
struct B : A { virtual void f(); };
struct D : B { void f() override; using B::f; };
}
void h(n::D& d) { d.f(); }
