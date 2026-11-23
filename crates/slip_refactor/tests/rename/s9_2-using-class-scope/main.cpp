namespace n {
struct B { protected: void f(int); };
struct D : B { public: using B::f; };
}
void h(n::D& d) { d.f(1); }
