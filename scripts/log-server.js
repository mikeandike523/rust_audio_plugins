const express = require('express');
const pino = require('pino');

const logger = pino({
  transport: {
    target: 'pino-pretty',
    options: { colorize: true }
  }
});

const app = express();
app.use(express.json());

app.post('/log', (req, res) => {
  logger.info(req.body);
  res.sendStatus(200);
});

const PORT = process.env.PORT || 9099;
app.listen(PORT, () => {
  console.log(`Logging server running on http://localhost:${PORT}`);
});
