namespace n {
struct B { void f(); void f(int); };
struct D : B { void f(); using B::f; };
}
void h(n::D& d) { d.f(); d.f(1); }
