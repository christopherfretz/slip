namespace n { template <class T> struct S { void f(); void k() { f(); } }; template <class T> void S<T>::f() {} }
