export class ClientContractError extends Error {
  readonly code: string;
  readonly retryable: boolean;
  readonly suggestedAction: string;

  constructor(code: string, message: string, retryable = false, suggestedAction = "检查输入后重试") {
    super(message);
    this.name = "ClientContractError";
    this.code = code;
    this.retryable = retryable;
    this.suggestedAction = suggestedAction;
  }
}
