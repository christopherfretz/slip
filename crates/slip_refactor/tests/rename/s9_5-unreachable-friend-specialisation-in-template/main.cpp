namespace n {
template <class T> void f(T) {}
template <class T> class A { friend void f<>(T); T x{}; };
class B { friend void f<>(int); friend void f<long>(long); };
}
n::A<int> a;
