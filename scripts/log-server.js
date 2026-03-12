const express = require('express');
const pino = require('pino');

const logger = pino({
  transport: {
    target: 'pino-pretty',
    options: { colorize: true }
  }
});

const app = express();

app.use((req, res, next) => {
  res.header('Access-Control-Allow-Origin', '*');
  res.header('Access-Control-Allow-Methods', 'POST, OPTIONS');
  res.header('Access-Control-Allow-Headers', 'Content-Type');

  if (req.method === 'OPTIONS') {
    res.sendStatus(204);
    return;
  }

  next();
});

app.use(express.json());

app.post('/log', (req, res) => {
  logger.info(req.body);
  res.sendStatus(200);
});

const PORT = process.env.PORT || 9099;
app.listen(PORT, () => {
  console.log(`Logging server running on http://localhost:${PORT}`);
});
