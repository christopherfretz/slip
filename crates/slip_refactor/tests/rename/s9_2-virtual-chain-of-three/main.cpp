namespace n {
struct B { virtual void f(); };
struct C : B { void f() override; };
struct D : C { void f() override; };
}
void h(n::D& d) { d.f(); }
