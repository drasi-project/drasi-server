MATCH (t:GpuSample)
WITH t, drasi.changeDateTime(t) AS last_seen
WHERE drasi.trueNowOrLater(
    last_seen + duration({ seconds: 5 }) <= datetime.realtime(),
    last_seen + duration({ seconds: 5 })
)
RETURN t.gpu_id AS gpu_id
