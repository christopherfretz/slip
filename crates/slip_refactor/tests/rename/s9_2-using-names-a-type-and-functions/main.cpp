namespace n {
struct B { struct f {}; void f(); };
struct D : B { using B::f; };
}
void h(n::D& d) { d.f(); }
