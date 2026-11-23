namespace n { template <class T> void f(T) {} template <class T> class A { friend void f<>(T); }; }
