// §10.7 ModuleTable `explicit_defs`: the unit holding an explicit instantiation definition. In one translation unit it is present exactly when the instantiation is a definition, so `SpecInfo` `kind` differs as well.
export module m;
export template <class T> void f(T) {}
extern template void f<int>(int);
