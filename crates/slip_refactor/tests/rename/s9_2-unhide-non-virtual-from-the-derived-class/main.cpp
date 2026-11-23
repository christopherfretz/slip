namespace n {
struct B { void f(); void f(int); };
struct D : B { using B::f; void f(); };
}
void h(n::D& d) { d.f(); d.f(1); }
