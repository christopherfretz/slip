namespace n {
struct B { virtual void f(); };
template <class T> struct D;
template <class T> struct D<T*> : B { void f() override; };
}
void h(n::D<int*>& d) { d.f(); }
