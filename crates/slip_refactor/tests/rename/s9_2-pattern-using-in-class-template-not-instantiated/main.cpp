namespace n {
struct Base { void f(); };
template <class T> struct F : Base { using Base::f; };
}
