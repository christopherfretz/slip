namespace n {
struct A { void f(); void f(int); };
struct B : A { };
struct D : B { void f(); using B::f; };
}
void h(n::D& d) { d.f(); d.f(1); }
