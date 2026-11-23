namespace n {
struct A { virtual void f(); };
struct B : A { void f() override; void f(int); };
struct D : B { using B::f; void f(int); };
}
void h(n::D& d) { d.f(); d.f(1); }
