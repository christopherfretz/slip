namespace n {
struct B { virtual void f(); };
template <class T> struct G : T { void f() override; };
}
