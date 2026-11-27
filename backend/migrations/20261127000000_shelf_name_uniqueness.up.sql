CREATE FUNCTION public.immutable_trim_unicode_whitespace(text) RETURNS text
LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE
AS $$
    SELECT btrim(
        $1,
        U&'\0009\000A\000B\000C\000D\0020\0085\00A0\1680\2000\2001\2002\2003\2004\2005\2006\2007\2008\2009\200A\2028\2029\202F\205F\3000'
    )
$$;

CREATE UNIQUE INDEX idx_shelves_user_name_folded
ON public.shelves (user_id, lower(public.immutable_unaccent(public.immutable_trim_unicode_whitespace(name))));
