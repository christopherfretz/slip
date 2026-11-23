namespace n {
struct B { void f(int); };
template <class T> struct D : B { using B::f; };
}
void h(n::D<int>& d) { d.f(1); }
