namespace n {
struct B { virtual void f(); };
template <class T> struct D : T { void f(); };
}
void h(n::D<n::B>& d) { d.f(); }
