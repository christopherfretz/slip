namespace n {
struct Base { virtual void f(); };
template <class T> struct E : Base { void f() override; };
}
