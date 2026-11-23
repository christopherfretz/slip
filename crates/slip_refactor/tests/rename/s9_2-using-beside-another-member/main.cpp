namespace n {
struct B { void f(); void f(int); };
struct D : B { using B::f; void f(double); };
}
void h(n::D& d) { d.f(1.0); d.f(1); }
