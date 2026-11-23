namespace n {
struct Base { void f(); };
template <class T> struct F : Base { using Base::f; };
}
void h(n::F<int>& x) { x.f(); }
