namespace n {
struct Base { virtual void f(); };
template <class T> struct E : Base { void f() override; };
}
void h(n::E<int>& e, n::E<long>& l) { e.f(); l.f(); }
